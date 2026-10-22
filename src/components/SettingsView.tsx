import {
  SettingsContainer,
  type SettingsContainerProps,
} from "./settings/SettingsContainer";

export type SettingsViewProps = SettingsContainerProps;

export function SettingsView(props: SettingsViewProps) {
  return <SettingsContainer {...props} />;
}
