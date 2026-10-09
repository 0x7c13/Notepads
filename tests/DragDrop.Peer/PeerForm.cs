// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Text;
using System.Windows.Forms;

namespace NotepadsDragDropPeer;

internal sealed class PeerForm : Form
{
    private readonly PeerChannel channel;
    private readonly TextPanel source = new("Drag source");
    private readonly TextPanel target = new("Drop receiver") { AllowDrop = true };
    private readonly System.Windows.Forms.Timer commands = new() { Interval = 40 };
    private readonly HashSet<string> cases = new(StringComparer.Ordinal);
    private string caseId = "";
    private string sourceText = "";
    private string targetText = "";
    private string receivedText = "";
    private string sourceMode = "normal";
    private string receiverMode = "normal";
    private string sourceError = "";
    private string targetError = "";
    private DragDropEffects allowed = DragDropEffects.Copy | DragDropEffects.Move;
    private DragDropEffects sourceEffect;
    private DragDropEffects targetEffect;
    private DragDropEffects targetAllowed;
    private DragDropEffects targetProvisional;
    private int targetKeyState;
    private bool sourceStarted;
    private bool sourceCompleted;
    private bool sourceEscapeSeen;
    private bool sourceActive;
    private bool targetDropped;
    private bool targetActive;
    private Point? mouseDown;
    private int nextCommand = 1;
    private bool processingCommand;
    private long lastActivity = Environment.TickCount64;

    internal PeerForm(PeerChannel channel)
    {
        this.channel = channel;
        Text = "Notepads DragDrop Test Peer";
        StartPosition = FormStartPosition.Manual;
        AutoScaleMode = AutoScaleMode.Dpi;
        AutoScaleDimensions = new SizeF(96, 96);
        ClientSize = new Size(450, 340);
        Location = new Point(30, 30);

        TableLayoutPanel layout = new()
        {
            Dock = DockStyle.Fill,
            ColumnCount = 1,
            RowCount = 2,
            Padding = new Padding(8)
        };
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 50));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 50));
        layout.Controls.Add(source, 0, 0);
        layout.Controls.Add(target, 0, 1);
        Controls.Add(layout);

        source.MouseDown += SourceMouseDown;
        source.MouseMove += SourceMouseMove;
        source.MouseUp += (_, _) => { mouseDown = null; source.Capture = false; };
        source.MouseCaptureChanged += (_, _) => { if (!source.Capture) mouseDown = null; };
        source.QueryContinueDrag += SourceQueryContinueDrag;
        target.DragEnter += TargetDragEnter;
        target.DragOver += TargetDragOver;
        target.DragDrop += TargetDragDrop;
        target.DragLeave += (_, _) =>
        {
            if (caseId.Length > 0) channel.Event(caseId, "target-drag-leave");
        };
        commands.Tick += PollCommands;
        Shown += (_, _) =>
        {
            channel.WriteAtomic("ready.txt", State());
            commands.Start();
        };
    }

    private void PollCommands(object? sender, EventArgs args)
    {
        if (processingCommand) return;
        if (!sourceActive && !targetActive && Environment.TickCount64 - lastActivity > 600_000)
        {
            Close();
            return;
        }

        if (nextCommand > 1000)
        {
            if (!sourceActive && !targetActive) Close();
            return;
        }

        string sequence = nextCommand.ToString("D4", CultureInfo.InvariantCulture);
        string? command = channel.Read("command-" + sequence + ".txt");
        if (command is null) return;
        processingCommand = true;
        try
        {
            lastActivity = Environment.TickCount64;
            string[] parts = command.TrimEnd('\r', '\n').Split('|');
            if (parts.Length == 1 && parts[0] == "state")
            {
                // Coordinates are measured here, after any driver window movement.
            }
            else if (parts.Length == 7 && parts[0] == "reset")
            {
                Reset(parts);
            }
            else
            {
                throw new InvalidDataException("Unknown or malformed peer command.");
            }

            channel.WriteAtomic("response-" + sequence + ".txt", State());
            nextCommand++;
        }
        catch (Exception exception)
        {
            channel.WriteAtomic("response-" + sequence + ".txt", "ERROR\r\nmessage64=" + PeerChannel.Encode(exception.ToString()) + "\r\n");
            throw;
        }
        finally
        {
            processingCommand = false;
        }
    }

    private void Reset(string[] parts)
    {
        if (sourceActive || targetActive)
        {
            throw new InvalidOperationException("Cannot reset during a drag callback.");
        }

        if (!Guid.TryParseExact(parts[1], "N", out Guid parsedCase) || parsedCase.ToString("N") != parts[1] || cases.Contains(parts[1]))
        {
            throw new InvalidDataException("Every reset requires a new lowercase GUID N case identifier.");
        }

        string newSource = PeerChannel.Decode(parts[2]);
        string newTarget = PeerChannel.Decode(parts[3]);
        if (parts[4] is not "normal" and not "delayed" and not "fail" ||
            parts[5] is not "normal" and not "delayed" and not "reject" ||
            parts[6] is not "1" and not "2" and not "3")
        {
            throw new InvalidDataException("Invalid mode or allowed effect mask.");
        }

        caseId = parts[1];
        cases.Add(caseId);
        sourceText = newSource;
        targetText = newTarget;
        sourceMode = parts[4];
        receiverMode = parts[5];
        allowed = (DragDropEffects)int.Parse(parts[6], CultureInfo.InvariantCulture);
        receivedText = sourceError = targetError = "";
        sourceEffect = targetEffect = targetAllowed = targetProvisional = DragDropEffects.None;
        targetKeyState = 0;
        sourceStarted = sourceCompleted = sourceEscapeSeen = targetDropped = false;
        mouseDown = null;
        UpdateModels();
        channel.Event(caseId, "reset", detail: "allowed=" + (int)allowed + ";source=" + sourceMode + ";receiver=" + receiverMode);
    }

    private void SourceMouseDown(object? sender, MouseEventArgs args)
    {
        if (args.Button != MouseButtons.Left || caseId.Length == 0 || sourceActive || targetActive || sourceText.Length == 0) return;
        source.Focus();
        mouseDown = args.Location;
        source.Capture = true;
    }

    private void SourceMouseMove(object? sender, MouseEventArgs args)
    {
        if (mouseDown is not Point origin || (args.Button & MouseButtons.Left) == 0) return;
        Size dragSize = SystemInformation.DragSize;
        Rectangle threshold = new(origin.X - dragSize.Width / 2, origin.Y - dragSize.Height / 2, dragSize.Width, dragSize.Height);
        if (threshold.Contains(args.Location)) return;
        mouseDown = null;
        source.Capture = false;
        sourceActive = true;
        lastActivity = Environment.TickCount64;
        channel.Event(caseId, "source-invoking", detail: "allowed=" + (int)allowed);
        try
        {
            GatedTextDataObject provider = new(channel, caseId, sourceText, sourceMode);
            sourceEffect = source.DoDragDrop(new DataObject(provider), allowed);
            if (sourceEffect == DragDropEffects.Move)
            {
                sourceText = "";
            }
        }
        catch (Exception exception)
        {
            sourceEffect = DragDropEffects.None;
            sourceError = exception.ToString();
            channel.Event(caseId, "source-error", detail: sourceError);
        }
        finally
        {
            sourceActive = false;
            sourceCompleted = true;
            lastActivity = Environment.TickCount64;
            UpdateModels();
            channel.Event(caseId, "source-completed", detail: "effect=" + (int)sourceEffect);
            channel.WriteAtomic(caseId + ".source-completed", State());
        }
    }

    private void SourceQueryContinueDrag(object? sender, QueryContinueDragEventArgs args)
    {
        if (!sourceStarted)
        {
            // This callback establishes that the real OLE loop has started; a mouse threshold
            // or a call about to enter DoDragDrop alone is not drag-start evidence.
            sourceStarted = true;
            channel.Event(caseId, "source-started", detail: "allowed=" + (int)allowed + ";keyState=" + args.KeyState);
            channel.WriteAtomic(caseId + ".source-started", State());
        }

        if (args.EscapePressed && !sourceEscapeSeen)
        {
            sourceEscapeSeen = true;
            channel.Event(caseId, "source-escape", detail: args.Action.ToString());
        }
    }

    private void TargetDragEnter(object? sender, DragEventArgs args)
    {
        if (caseId.Length == 0)
        {
            args.Effect = DragDropEffects.None;
            return;
        }

        targetAllowed = args.AllowedEffect;
        targetKeyState = args.KeyState;
        bool unicodeAvailable = args.Data?.GetDataPresent(DataFormats.UnicodeText, false) == true;
        targetProvisional = unicodeAvailable ? ChooseEffect(args) : DragDropEffects.None;
        args.Effect = targetProvisional;
        channel.Event(caseId, "target-drag-enter", detail: "allowed=" + (int)targetAllowed + ";keyState=" + targetKeyState + ";effect=" + (int)args.Effect);
    }

    private void TargetDragOver(object? sender, DragEventArgs args)
    {
        DragDropEffects effect = caseId.Length > 0 && args.Data?.GetDataPresent(DataFormats.UnicodeText, false) == true
            ? ChooseEffect(args) : DragDropEffects.None;
        if (caseId.Length > 0 && (targetProvisional != effect || targetKeyState != args.KeyState))
        {
            channel.Event(caseId, "target-drag-over", detail: "keyState=" + args.KeyState + ";effect=" + (int)effect);
        }

        targetAllowed = args.AllowedEffect;
        targetKeyState = args.KeyState;
        targetProvisional = effect;
        args.Effect = effect;
    }

    private DragDropEffects ChooseEffect(DragEventArgs args)
    {
        if (receiverMode == "reject") return DragDropEffects.None;
        if ((args.KeyState & 8) != 0) return args.AllowedEffect & DragDropEffects.Copy;
        if ((args.KeyState & 4) != 0) return args.AllowedEffect & DragDropEffects.Move;
        return (args.AllowedEffect & DragDropEffects.Move) != 0 ? DragDropEffects.Move : args.AllowedEffect & DragDropEffects.Copy;
    }

    private void TargetDragDrop(object? sender, DragEventArgs args)
    {
        args.Effect = DragDropEffects.None;
        if (caseId.Length == 0) return;
        targetActive = true;
        targetDropped = true;
        targetEffect = DragDropEffects.None;
        targetAllowed = args.AllowedEffect;
        targetKeyState = args.KeyState;
        lastActivity = Environment.TickCount64;
        channel.Event(caseId, "target-drop-callback-enter");
        try
        {
            DragDropEffects effect = ChooseEffect(args);
            if (effect == DragDropEffects.None || args.Data is null) return;
            // Request exactly CF_UNICODETEXT with no ANSI/serialized fallback. This handler
            // stays synchronous so the OLE source cannot finish while the drop is gated.
#pragma warning disable WFDEV005 // Intentionally exercise IDataObject Unicode retrieval across OLE processes.
            object? data = args.Data.GetData(DataFormats.UnicodeText, false);
#pragma warning restore WFDEV005
            if (data is not string payload)
            {
                throw new InvalidDataException("The source did not provide Unicode text.");
            }

            receivedText = payload;
            channel.Event(caseId, "target-drop-entered", detail: "received64=" + PeerChannel.Encode(payload));
            channel.WriteAtomic(caseId + ".target-drop-entered", State());
            if (receiverMode == "delayed" && !channel.WaitForRelease(caseId, "release-drop", 0)) return;

            targetText += payload;
            targetEffect = effect;
            args.Effect = effect;
            UpdateModels();
        }
        catch (Exception exception)
        {
            targetError = exception.ToString();
            targetEffect = args.Effect = DragDropEffects.None;
            channel.Event(caseId, "target-error", detail: targetError);
        }
        finally
        {
            targetActive = false;
            lastActivity = Environment.TickCount64;
            channel.Event(caseId, "target-completed", detail: "effect=" + (int)targetEffect);
            channel.WriteAtomic(caseId + ".target-completed", State());
        }
    }

    private void UpdateModels()
    {
        source.Model = sourceText;
        target.Model = targetText;
    }

    private string State()
    {
        StringBuilder result = new("OK\r\n");
        void Add(string key, object value) => result.Append(key).Append('=').Append(Convert.ToString(value, CultureInfo.InvariantCulture)).Append("\r\n");
        Add("ticks", DateTime.UtcNow.Ticks);
        Add("source64", PeerChannel.Encode(sourceText));
        Add("target64", PeerChannel.Encode(targetText));
        Add("received64", PeerChannel.Encode(receivedText));
        Add("sourceEffect", (int)sourceEffect);
        Add("targetEffect", (int)targetEffect);
        Add("sourceStarted", sourceStarted ? 1 : 0);
        Add("sourceCompleted", sourceCompleted ? 1 : 0);
        Add("sourceEscapeSeen", sourceEscapeSeen ? 1 : 0);
        Add("sourceActive", sourceActive ? 1 : 0);
        Add("targetDropped", targetDropped ? 1 : 0);
        Add("targetActive", targetActive ? 1 : 0);
        Add("allowed", (int)allowed);
        Add("targetAllowed", (int)targetAllowed);
        Add("targetKeyState", targetKeyState);
        Add("targetProvisional", (int)targetProvisional);
        Add("sourceError64", PeerChannel.Encode(sourceError));
        Add("targetError64", PeerChannel.Encode(targetError));
        Add("sourceMode", sourceMode);
        Add("receiverMode", receiverMode);
        AddBounds("source", source);
        AddBounds("target", target);
        Add("hwnd", Handle.ToInt64());
        Add("pid", Environment.ProcessId);
        Add("case", caseId);
        return result.ToString();

        void AddBounds(string prefix, Control control)
        {
            // PerMonitorV2 is enabled before handles exist. PointToScreen and actual client
            // size therefore describe physical screen pixels, including after repositioning.
            Point position = control.PointToScreen(Point.Empty);
            Add(prefix + "X", position.X);
            Add(prefix + "Y", position.Y);
            Add(prefix + "Width", control.ClientSize.Width);
            Add(prefix + "Height", control.ClientSize.Height);
            Add(prefix + "Hwnd", control.Handle.ToInt64());
        }
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            commands.Dispose();
            source.Dispose();
            target.Dispose();
        }

        base.Dispose(disposing);
    }

    private sealed class TextPanel : Control
    {
        private readonly string heading;
        private string model = "";

        internal TextPanel(string heading)
        {
            this.heading = heading;
            Dock = DockStyle.Fill;
            Margin = new Padding(4);
            BackColor = SystemColors.Window;
            ForeColor = SystemColors.WindowText;
            TabStop = true;
            SetStyle(ControlStyles.Selectable | ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer, true);
        }

        [DesignerSerializationVisibility(DesignerSerializationVisibility.Hidden)]
        internal string Model
        {
            set { model = value; Invalidate(); }
        }

        protected override void OnPaint(PaintEventArgs args)
        {
            base.OnPaint(args);
            ControlPaint.DrawBorder(args.Graphics, ClientRectangle, SystemColors.WindowFrame, ButtonBorderStyle.Solid);
            Rectangle bounds = Rectangle.Inflate(ClientRectangle, -8, -8);
            TextRenderer.DrawText(args.Graphics, heading + "\r\n\r\n" + model, Font, bounds, ForeColor,
                TextFormatFlags.NoPrefix | TextFormatFlags.WordBreak | TextFormatFlags.TextBoxControl);
        }
    }
}
