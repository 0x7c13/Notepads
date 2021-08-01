namespace Notepads.Utilities
{
    using System;
    using Windows.UI.Xaml;
    using Microsoft.Xaml.Interactivity;

    public static class BehaviorUtility
    {
        public static DataTemplate GetAttachedBehaviors(DependencyObject obj)
        {
            return (DataTemplate)obj.GetValue(AttachedBehaviorsProperty);
        }

        public static void SetAttachedBehaviors(DependencyObject obj, DataTemplate value)
        {
            obj.SetValue(AttachedBehaviorsProperty, value);
        }

        public static readonly DependencyProperty AttachedBehaviorsProperty =
            DependencyProperty.RegisterAttached(
                "AttachedBehaviors",
                typeof(DataTemplate),
                typeof(BehaviorUtility),
                new PropertyMetadata(null, Callback)
            );

        private static void Callback(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            BehaviorCollection collection = null;
            var value = (e.NewValue as DataTemplate)?.LoadContent();

            switch (value)
            {
                case BehaviorCollection behaviors:
                    collection = behaviors;
                    break;
                case IBehavior _:
                    collection = new BehaviorCollection { value };
                    break;
                default:
                    throw new Exception($"AttachedBehaviors should be a BehaviorCollection or an IBehavior.");
            }
            // collection may be null here, if e.NewValue is null
            Interaction.SetBehaviors(d, collection);
        }
    }
}
