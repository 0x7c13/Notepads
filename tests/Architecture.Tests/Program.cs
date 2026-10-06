// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------


using System.Collections.Immutable;
using System.Reflection;
using System.Reflection.Emit;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;

namespace NotepadsArchitectureTests;

internal static class Program
{
    private static int Main(string[] args)
    {
        if (args.Length == 1 && args[0] == "--self-test")
        {
            var violations = Inspect(Assembly.GetExecutingAssembly().Location);
            foreach (var kind in new[] { "Field", "Signature", "Property", "Call", "Attribute", "Interface", "Xaml", "UnknownNamespace" })
            {
                if (!violations.Any(v => v.Contains("Forbidden" + kind)))
                    throw new InvalidOperationException("The dependency probe escaped detection: " + kind);
            }

            Console.WriteLine("PASS: architecture guard rejects forbidden fields, generic signatures, properties, IL calls, attributes, interfaces, external XAML and unknown application namespaces.");
            return 0;
        }
        if (args.Length != 1) throw new ArgumentException("Pass the managed Notepads.dll before Native AOT compilation, or --self-test.");
        var failures = Inspect(Path.GetFullPath(args[0]));
        foreach (var failure in failures) Console.Error.WriteLine(failure);
        if (failures.Count != 0) return 1;
        Console.WriteLine("PASS: compiled Notepads dependency graph follows the declared layers and is acyclic.");
        return 0;
    }

    private static SortedSet<string> Inspect(string path)
    {
        using var stream = File.OpenRead(path);
        using var pe = new PEReader(stream);
        var reader = pe.GetMetadataReader();
        var provider = new TypeReferences(reader);
        var failures = new SortedSet<string>(StringComparer.Ordinal);
        var graph = new Dictionary<string, HashSet<string>>();
        var opcodes = typeof(OpCodes).GetFields(BindingFlags.Static | BindingFlags.Public)
            .Select(f => (OpCode)f.GetValue(null)!).ToDictionary(o => unchecked((ushort)o.Value));

        void Check(string source, IEnumerable<string> targets)
        {
            foreach (var target in targets)
            {
                if (!LayerRules.Allows(source, target)) failures.Add(source + " -> " + target);
                var from = LayerRules.Module(source);
                var to = LayerRules.Module(target);
                if (from == null || to == null || from == to || to is "Native" or "ExternalControls") continue;
                if (!graph.TryGetValue(from, out var edges)) graph[from] = edges = new HashSet<string>();
                edges.Add(to);
            }
        }

        void Attributes(string source, CustomAttributeHandleCollection attributes)
        {
            foreach (var handle in attributes)
            {
                var attribute = reader.GetCustomAttribute(handle);
                Check(source, provider.Resolve(attribute.Constructor));
                var value = attribute.DecodeValue(provider);
                foreach (var argument in value.FixedArguments) AttributeArgument(source, argument.Type, argument.Value);
                foreach (var argument in value.NamedArguments) AttributeArgument(source, argument.Type, argument.Value);
            }
        }

        void AttributeArgument(string source, HashSet<string> type, object? value)
        {
            Check(source, type);
            if (value is HashSet<string> types) Check(source, types);
            if (value is ImmutableArray<CustomAttributeTypedArgument<HashSet<string>>> array)
                foreach (var item in array) AttributeArgument(source, item.Type, item.Value);
        }

        void Constraints(string source, GenericParameterHandleCollection parameters)
        {
            foreach (var handle in parameters)
            {
                foreach (var constraint in reader.GetGenericParameter(handle).GetConstraints())
                    Check(source, provider.Resolve(reader.GetGenericParameterConstraint(constraint).Type));
            }
        }

        foreach (var handle in reader.TypeDefinitions)
        {
            var type = reader.GetTypeDefinition(handle);
            var source = provider.DefinitionName(handle);
            if (LayerRules.Module(source) == "UnknownApp") failures.Add("Undeclared app module: " + source);
            if (LayerRules.Module(source) == null) continue;
            Check(source, provider.Resolve(type.BaseType));
            Attributes(source, type.GetCustomAttributes());
            Constraints(source, type.GetGenericParameters());
            foreach (var implementation in type.GetInterfaceImplementations())
                Check(source, provider.Resolve(reader.GetInterfaceImplementation(implementation).Interface));
            foreach (var field in type.GetFields())
            {
                var definition = reader.GetFieldDefinition(field);
                Check(source, definition.DecodeSignature(provider, null));
                Attributes(source, definition.GetCustomAttributes());
            }
            foreach (var property in type.GetProperties())
            {
                var definition = reader.GetPropertyDefinition(property);
                Check(source, TypeReferences.Signature(definition.DecodeSignature(provider, null)));
                Attributes(source, definition.GetCustomAttributes());
            }
            foreach (var eventHandle in type.GetEvents())
            {
                var definition = reader.GetEventDefinition(eventHandle);
                Check(source, provider.Resolve(definition.Type));
                Attributes(source, definition.GetCustomAttributes());
            }
            foreach (var method in type.GetMethods())
            {
                var definition = reader.GetMethodDefinition(method);
                Check(source, TypeReferences.Signature(definition.DecodeSignature(provider, null)));
                Attributes(source, definition.GetCustomAttributes());
                Constraints(source, definition.GetGenericParameters());
                foreach (var parameter in definition.GetParameters()) Attributes(source, reader.GetParameter(parameter).GetCustomAttributes());
                if (definition.RelativeVirtualAddress == 0) continue;
                var body = pe.GetMethodBody(definition.RelativeVirtualAddress);
                if (!body.LocalSignature.IsNil)
                    foreach (var local in reader.GetStandaloneSignature(body.LocalSignature).DecodeLocalSignature(provider, null)) Check(source, local);
                foreach (var region in body.ExceptionRegions) Check(source, provider.Resolve(region.CatchType));
                var il = body.GetILBytes()!;
                var offset = 0;
                while (offset < il.Length)
                {
                    ushort code = il[offset++];
                    if (code == 0xfe) code = (ushort)(0xfe00 | il[offset++]);
                    var opcode = opcodes[code];
                    switch (opcode.OperandType)
                    {
                        case OperandType.InlineField:
                        case OperandType.InlineMethod:
                        case OperandType.InlineType:
                        case OperandType.InlineTok:
                        case OperandType.InlineSig:
                            Check(source, provider.Resolve(System.Reflection.Metadata.Ecma335.MetadataTokens.EntityHandle(BitConverter.ToInt32(il, offset))));
                            offset += 4;
                            break;
                        case OperandType.InlineSwitch:
                            offset += 4 + 4 * BitConverter.ToInt32(il, offset);
                            break;
                        case OperandType.InlineI8:
                        case OperandType.InlineR: offset += 8; break;
                        case OperandType.InlineBrTarget:
                        case OperandType.InlineI:
                        case OperandType.InlineString:
                        case OperandType.ShortInlineR: offset += 4; break;
                        case OperandType.InlineVar: offset += 2; break;
                        case OperandType.ShortInlineBrTarget:
                        case OperandType.ShortInlineI:
                        case OperandType.ShortInlineVar: offset++; break;
                        case OperandType.InlineNone: break;
                        default: throw new InvalidDataException("Unsupported IL operand: " + opcode);
                    }
                }
            }
        }
        var visiting = new HashSet<string>();
        var visited = new HashSet<string>();
        void Visit(string module)
        {
            if (visited.Contains(module)) return;
            if (!visiting.Add(module)) { failures.Add("Module cycle at " + module); return; }
            if (graph.TryGetValue(module, out var edges)) foreach (var edge in edges) Visit(edge);
            visiting.Remove(module);
            visited.Add(module);
        }
        foreach (var module in graph.Keys) Visit(module);
        return failures;
    }

    private sealed class TypeReferences(MetadataReader reader) :
        ISignatureTypeProvider<HashSet<string>, object?>, ICustomAttributeTypeProvider<HashSet<string>>
    {
        public string DefinitionName(TypeDefinitionHandle handle)
        {
            var definition = reader.GetTypeDefinition(handle);
            var parent = definition.GetDeclaringType();
            return parent.IsNil ? reader.GetString(definition.Namespace) + "." + reader.GetString(definition.Name) :
                DefinitionName(parent) + "+" + reader.GetString(definition.Name);
        }

        public HashSet<string> Resolve(EntityHandle handle)
        {
            if (handle.IsNil) return [];
            switch (handle.Kind)
            {
                case HandleKind.TypeDefinition: return [DefinitionName((TypeDefinitionHandle)handle)];
                case HandleKind.TypeReference: return GetTypeFromReference(reader, (TypeReferenceHandle)handle, 0);
                case HandleKind.TypeSpecification: return reader.GetTypeSpecification((TypeSpecificationHandle)handle).DecodeSignature(this, null);
                case HandleKind.MethodDefinition:
                    var method = reader.GetMethodDefinition((MethodDefinitionHandle)handle);
                    return Merge(Resolve(method.GetDeclaringType()), Signature(method.DecodeSignature(this, null)));
                case HandleKind.FieldDefinition:
                    var field = reader.GetFieldDefinition((FieldDefinitionHandle)handle);
                    return Merge(Resolve(field.GetDeclaringType()), field.DecodeSignature(this, null));
                case HandleKind.MemberReference:
                    var member = reader.GetMemberReference((MemberReferenceHandle)handle);
                    return Merge(Resolve(member.Parent), member.GetKind() == MemberReferenceKind.Field ?
                        member.DecodeFieldSignature(this, null) : Signature(member.DecodeMethodSignature(this, null)));
                case HandleKind.MethodSpecification:
                    var specification = reader.GetMethodSpecification((MethodSpecificationHandle)handle);
                    return Merge(Resolve(specification.Method), specification.DecodeSignature(this, null).SelectMany(t => t));
                case HandleKind.StandaloneSignature:
                    return Signature(reader.GetStandaloneSignature((StandaloneSignatureHandle)handle).DecodeMethodSignature(this, null));
                default: return [];
            }
        }

        public static HashSet<string> Signature(MethodSignature<HashSet<string>> signature) => Merge(signature.ReturnType, signature.ParameterTypes.SelectMany(t => t));
        private static HashSet<string> Merge(IEnumerable<string> first, IEnumerable<string> second) => new(first.Concat(second));
        public HashSet<string> GetTypeFromDefinition(MetadataReader metadata, TypeDefinitionHandle handle, byte rawKind) => Resolve(handle);
        public HashSet<string> GetTypeFromReference(MetadataReader metadata, TypeReferenceHandle handle, byte rawKind)
        {
            var type = metadata.GetTypeReference(handle);
            var parent = type.ResolutionScope;
            return parent.Kind == HandleKind.TypeReference ?
                [GetTypeFromReference(metadata, (TypeReferenceHandle)parent, 0).Single() + "+" + metadata.GetString(type.Name)] :
                [metadata.GetString(type.Namespace) + "." + metadata.GetString(type.Name)];
        }
        public HashSet<string> GetTypeFromSpecification(MetadataReader metadata, object? context, TypeSpecificationHandle handle, byte rawKind) => Resolve(handle);
        public HashSet<string> GetGenericInstantiation(HashSet<string> genericType, ImmutableArray<HashSet<string>> arguments) => Merge(genericType, arguments.SelectMany(t => t));
        public HashSet<string> GetArrayType(HashSet<string> type, ArrayShape shape) => type;
        public HashSet<string> GetByReferenceType(HashSet<string> type) => type;
        public HashSet<string> GetPointerType(HashSet<string> type) => type;
        public HashSet<string> GetSZArrayType(HashSet<string> type) => type;
        public HashSet<string> GetPinnedType(HashSet<string> type) => type;
        public HashSet<string> GetModifiedType(HashSet<string> modifier, HashSet<string> type, bool required) => Merge(modifier, type);
        public HashSet<string> GetFunctionPointerType(MethodSignature<HashSet<string>> signature) => Signature(signature);
        public HashSet<string> GetGenericMethodParameter(object? context, int index) => [];
        public HashSet<string> GetGenericTypeParameter(object? context, int index) => [];
        public HashSet<string> GetPrimitiveType(PrimitiveTypeCode code) => ["$" + code];
        public HashSet<string> GetSystemType() => ["System.Type"];
        public bool IsSystemType(HashSet<string> type) => type.Contains("System.Type");
        public HashSet<string> GetTypeFromSerializedName(string name) => new(System.Text.RegularExpressions.Regex.Matches(name, @"(?:Notepads|WinUIEditor)\.[\w.+`]+").Select(m => m.Value));
        public PrimitiveTypeCode GetUnderlyingEnumType(HashSet<string> type)
        {
            foreach (var handle in reader.TypeDefinitions)
            {
                if (!type.Contains(DefinitionName(handle))) continue;
                foreach (var field in reader.GetTypeDefinition(handle).GetFields())
                {
                    var definition = reader.GetFieldDefinition(field);
                    if (reader.GetString(definition.Name) == "value__")
                        return Enum.Parse<PrimitiveTypeCode>(definition.DecodeSignature(this, null).Single().TrimStart('$'));
                }
            }
            return PrimitiveTypeCode.Int32;
        }
    }
}
