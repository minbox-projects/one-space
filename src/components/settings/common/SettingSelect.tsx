import {
  CustomSelect,
  type CustomSelectOption,
  type CustomSelectProps,
} from "@/components/common/CustomSelect";

export type SettingSelectOption<T extends string = string> = CustomSelectOption<T>;
export type SettingSelectProps<T extends string = string> = CustomSelectProps<T>;

export function SettingSelect<T extends string = string>(props: SettingSelectProps<T>) {
  return <CustomSelect<T> {...props} />;
}
