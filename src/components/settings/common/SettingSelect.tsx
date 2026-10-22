import {
  CustomSelect,
  type CustomSelectProps,
} from "@/components/common/CustomSelect";

export type SettingSelectProps<T extends string = string> = CustomSelectProps<T>;

export function SettingSelect<T extends string = string>(props: SettingSelectProps<T>) {
  return <CustomSelect<T> {...props} />;
}
