# ---------------------------------------------------------------------------------------------
#  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
#  See LICENSE file in the project root for license information.
# ---------------------------------------------------------------------------------------------

[CmdletBinding()]
param(
    [string] $ExpectedPath = (Join-Path $PSScriptRoot '../expected/independent-goldens.json'),
    [string] $ReportPath = (Join-Path $PSScriptRoot '../coordinate-check.json')
)

$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false, $true)
$fixtureRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $fixtureRoot '../../..'))

# This checker only validates already authored numeric data and string identity.
# It does not recognize Markdown, normalize input, construct fragments, or calculate relations.
function Get-ScalarLedger([string] $Value)
{
    $records = [Collections.Generic.List[object]]::new()
    $byteBoundaries = [Collections.Generic.List[long]]::new()
    $utf16Boundaries = [Collections.Generic.List[long]]::new()
    $byteBoundaries.Add(0)
    $utf16Boundaries.Add(0)
    $byteOffset = 0L
    for ($offset = 0; $offset -lt $Value.Length;)
    {
        $units = 1
        if ([char]::IsHighSurrogate($Value[$offset]))
        {
            if ($offset + 1 -ge $Value.Length -or -not [char]::IsLowSurrogate($Value[$offset + 1]))
            {
                throw 'Unpaired UTF-16 high surrogate in authored data.'
            }
            $units = 2
        }
        elseif ([char]::IsLowSurrogate($Value[$offset]))
        {
            throw 'Unpaired UTF-16 low surrogate in authored data.'
        }
        $scalar = $Value.Substring($offset, $units)
        $bytes = $utf8.GetByteCount($scalar)
        $records.Add([ordered]@{
            codepoint = [char]::ConvertToUtf32($Value, $offset)
            utf8 = @($byteOffset, $byteOffset + $bytes)
            utf16 = @([long]$offset, [long]($offset + $units))
        })
        $offset += $units
        $byteOffset += $bytes
        $byteBoundaries.Add($byteOffset)
        $utf16Boundaries.Add([long]$offset)
    }
    return [ordered]@{
        bytes = $byteOffset
        utf16_units = $Value.Length
        utf8_scalar_boundaries = @($byteBoundaries)
        utf16_scalar_boundaries = @($utf16Boundaries)
        scalars = @($records)
    }
}

function Assert-Span($Span, $Boundaries, [string] $Context)
{
    if ($Span.Count -ne 2 -or $Span[0] -gt $Span[1] -or
        $Span[0] -notin $Boundaries -or $Span[1] -notin $Boundaries)
    {
        throw "Illegal authored span in ${Context}: $($Span -join ',')."
    }
}

function Get-ByteSlice([byte[]] $Bytes, $Span)
{
    return $utf8.GetString($Bytes, [int]$Span[0], [int]($Span[1] - $Span[0]))
}

$data = [IO.File]::ReadAllText([IO.Path]::GetFullPath($ExpectedPath), $utf8) | ConvertFrom-Json -Depth 100
if ($data.status -notin @('draft_requires_independent_review', 'approved_basic_oracle'))
{
    throw 'This checker accepts explicitly draft or independently approved basic oracle data; it cannot change approval or freeze an oracle.'
}
$caseIds = [Collections.Generic.HashSet[string]]::new()
$documents = [Collections.Generic.HashSet[string]]::new()
$caseReports = [Collections.Generic.List[object]]::new()
$totalQueries = 0
$totalFragments = 0
$totalPayloads = 0
$totalControls = 0
foreach ($case in $data.cases)
{
    if (-not $caseIds.Add($case.id) -or -not $documents.Add($case.source_key.document_id))
    {
        throw 'Duplicate case or document identity.'
    }
    if ($case.source_key.revision -lt 1 -or $case.source_key.document_id -notmatch '^[0-9a-f]{32}$')
    {
        throw "Invalid SourceKey in $($case.id)."
    }
    $source = Get-ScalarLedger $case.source_utf8
    $parse = Get-ScalarLedger $case.expected_input.parse_utf8
    $sourceBytes = $utf8.GetBytes($case.source_utf8)
    $parseBytes = $utf8.GetBytes($case.expected_input.parse_utf8)
    $inputPath = [IO.Path]::GetFullPath((Join-Path $fixtureRoot $case.input_path))
    if (-not $inputPath.StartsWith($fixtureRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))
    {
        throw 'Input path escapes fixture root.'
    }
    $actualInput = [IO.File]::ReadAllBytes($inputPath)
    if ([Convert]::ToHexString($actualInput) -cne [Convert]::ToHexString($sourceBytes))
    {
        throw "Original source file mismatch in $($case.id)."
    }
    $sourceCursor = 0L
    $parseCursor = 0L
    foreach ($piece in $case.expected_input.pieces)
    {
        Assert-Span $piece.source $source.utf8_scalar_boundaries "$($case.id) input S"
        Assert-Span $piece.parse $parse.utf8_scalar_boundaries "$($case.id) input P"
        if ($piece.source[0] -ne $sourceCursor -or $piece.parse[0] -ne $parseCursor -or
            $piece.source[0] -eq $piece.source[1] -or $piece.parse[0] -eq $piece.parse[1])
        {
            throw "Input piece partition mismatch in $($case.id)."
        }
        if ($piece.kind -eq 'Copy' -and
            (Get-ByteSlice $sourceBytes $piece.source) -cne (Get-ByteSlice $parseBytes $piece.parse))
        {
            throw "Authored input Copy is not identical in $($case.id)."
        }
        $sourceCursor = $piece.source[1]
        $parseCursor = $piece.parse[1]
    }
    if ($sourceCursor -ne $source.bytes -or $parseCursor -ne $parse.bytes)
    {
        throw "Incomplete authored input partition in $($case.id)."
    }
    $flowIds = @($case.expected_document.flows.id)
    $fragmentIds = [Collections.Generic.HashSet[long]]::new()
    $flowReports = [Collections.Generic.List[object]]::new()
    foreach ($flow in $case.expected_document.flows)
    {
        $logical = Get-ScalarLedger $flow.readable_utf8
        if ($flow.logical_utf16 -ne $logical.utf16_units -or $flow.source_domains.Count -ne 1)
        {
            throw "Logical count/domain mismatch in $($case.id) flow $($flow.id)."
        }
        $domain = $flow.source_domains[0]
        Assert-Span $domain $source.utf8_scalar_boundaries "$($case.id) domain"
        if ($flow.id -eq 0 -and ($domain[0] -ne 0 -or $domain[1] -ne $source.bytes))
        {
            throw "Main does not cover S in $($case.id)."
        }
        $sourceCursor = $domain[0]
        $logicalCursor = 0L
        $joined = [Text.StringBuilder]::new()
        foreach ($fragment in $flow.fragments)
        {
            if (-not $fragmentIds.Add([long]$fragment.id))
            {
                throw "Duplicate document fragment ID in $($case.id)."
            }
            Assert-Span $fragment.source $source.utf8_scalar_boundaries "$($case.id) fragment S"
            Assert-Span $fragment.logical $logical.utf16_scalar_boundaries "$($case.id) fragment L"
            if ($fragment.source[0] -ne $sourceCursor -or $fragment.source[1] -gt $domain[1] -or
                $fragment.logical[0] -ne $logicalCursor)
            {
                throw "Authored flow partition mismatch in $($case.id) fragment $($fragment.id)."
            }
            $output = switch ($fragment.kind)
            {
                'Copy' { Get-ByteSlice $sourceBytes $fragment.source }
                'Hidden' { '' }
                'Transform' { $fragment.output_utf8 }
                'Synthetic' { $fragment.output_utf8 }
                'Object' { [string][char]0xFFFC }
                default { throw "Unknown authored fragment kind $($fragment.kind)." }
            }
            if ($output.Length -ne $fragment.logical[1] - $fragment.logical[0])
            {
                throw "Authored output count mismatch in $($case.id) fragment $($fragment.id)."
            }
            if (($fragment.kind -eq 'Synthetic') -ne ($fragment.source[0] -eq $fragment.source[1]))
            {
                throw "Authored zero-width source misuse in $($case.id)."
            }
            [void]$joined.Append($output)
            $sourceCursor = $fragment.source[1]
            $logicalCursor = $fragment.logical[1]
        }
        if ($sourceCursor -ne $domain[1] -or $logicalCursor -ne $logical.utf16_units -or
            $joined.ToString() -cne $flow.readable_utf8)
        {
            throw "Authored readable/partition mismatch in $($case.id) flow $($flow.id)."
        }
        $flowReports.Add([ordered]@{ id=$flow.id; scope=$flow.scope; logical=$logical; fragments=$flow.fragments.Count })
        $totalFragments += $flow.fragments.Count
    }
    foreach ($payload in $case.expected_document.payloads)
    {
        $payloadLedger = Get-ScalarLedger $payload.decoded_utf8
        $payloadBytes = $utf8.GetBytes($payload.decoded_utf8)
        $cursor = 0L
        foreach ($part in $payload.source_parts)
        {
            Assert-Span $part.source $source.utf8_scalar_boundaries "$($case.id) payload source"
            Assert-Span $part.payload_utf8 $payloadLedger.utf8_scalar_boundaries "$($case.id) payload bytes"
            if ($part.payload_utf8[0] -ne $cursor -or $part.payload_utf8[0] -eq $part.payload_utf8[1])
            {
                throw "Authored payload partition mismatch in $($case.id)."
            }
            if (($part.kind -eq 'Synthetic') -ne ($part.source[0] -eq $part.source[1]))
            {
                throw "Authored payload source anchor mismatch in $($case.id)."
            }
            if ($part.kind -eq 'Copy' -and
                (Get-ByteSlice $sourceBytes $part.source) -cne (Get-ByteSlice $payloadBytes $part.payload_utf8))
            {
                throw "Authored payload Copy mismatch in $($case.id)."
            }
            $cursor = $part.payload_utf8[1]
        }
        if ($cursor -ne $payloadLedger.bytes)
        {
            throw "Incomplete authored payload in $($case.id)."
        }
        $totalPayloads++
    }
    foreach ($query in $case.queries)
    {
        if ($query.target.kind -ne 'Flow' -or $query.target.id -notin $flowIds -or -not $query.source_key)
        {
            throw "Missing authored query target/key in $($case.id)."
        }
        if ($query.expected)
        {
            $oppositeBoundaries = if ($query.direction -eq 'source_to_logical')
            {
                (Get-ScalarLedger ($case.expected_document.flows | Where-Object id -eq $query.target.id).readable_utf8).utf16_scalar_boundaries
            }
            else { $source.utf8_scalar_boundaries }
            Assert-Span $query.expected.envelope $oppositeBoundaries "$($case.id) query envelope"
            $edge = if ($query.affinity -eq 'Before') { 0 } else { 1 }
            if ($query.expected.chosen_position -ne $query.expected.envelope[$edge])
            {
                throw "Authored affinity/envelope endpoint mismatch in $($case.id)."
            }
            $targetFragments = @(($case.expected_document.flows | Where-Object id -eq $query.target.id).fragments.id)
            foreach ($contributor in $query.expected.contributors)
            {
                if ($contributor -notin $targetFragments)
                {
                    throw "Authored contributor belongs to a different flow in $($case.id)."
                }
            }
        }
        $totalQueries++
    }
    if ($case.fidelity_negative_controls) { $totalControls += $case.fidelity_negative_controls.Count }
    $caseReports.Add([ordered]@{
        id=$case.id; layer=$case.layer; source_key=$case.source_key
        source=$source; parse=$parse; flows=@($flowReports)
        input_sha256=(Get-FileHash -LiteralPath $inputPath -Algorithm SHA256).Hash
        queries=$case.queries.Count; payloads=$case.expected_document.payloads.Count
    })
}
$normativeChecks = foreach ($sourceRecord in $data.normative_sources)
{
    $path = Join-Path $repositoryRoot $sourceRecord.path
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    if ($actual -cne $sourceRecord.sha256)
    {
        throw "Pinned normative source hash mismatch: $($sourceRecord.path)."
    }
    [ordered]@{ path=$sourceRecord.path; sha256=$actual; bytes=(Get-Item -LiteralPath $path).Length }
}
$report = [ordered]@{
    schema_version='m1-e04-coordinate-check-research.1'
    data_version=$data.data_version
    status='counts_and_authored_data_identity_checked_only'
    limitations='No Markdown semantic decision, normalization output derivation, BoundaryRelation calculation, parser execution, implementation result, or oracle freeze is performed.'
    expected_sha256=(Get-FileHash -LiteralPath $ExpectedPath -Algorithm SHA256).Hash
    counts=[ordered]@{ cases=$caseReports.Count; queries=$totalQueries; fragments=$totalFragments; payloads=$totalPayloads; fidelity_negative_controls=$totalControls; pending=$data.pending.Count }
    normative_sources=@($normativeChecks)
    cases=@($caseReports)
}
$json = ($report | ConvertTo-Json -Depth 100).Replace("`r`n", "`n").Replace("`n", "`r`n") + "`r`n"
[IO.File]::WriteAllText([IO.Path]::GetFullPath($ReportPath), $json, $utf8)
$report.counts | ConvertTo-Json
