import { Select, Stack, TextInput, ToggleSwitch } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";

import { adminApi } from "@/api";
import type { SettingsDto } from "@floatctf/sdk";
import { FrontendSelector, GenericTable } from "@/components";
import { MyTruncate } from "@/components/Truncate";
import { SettingValueType } from "@floatctf/sdk/entity";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/settings")({
  component: RouteComponent,
  loader: AdminRouteGuard,
});

function RouteComponent() {
  useTitle("Settings | FloatCTF");
  const subject = "Settings";
  const columns = [
    { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
    {
      accessorKey: "key",
      header: "键",
      field: "key",
      label: "配置键",
      sortBy: true,
    },
    {
      accessorKey: "value",
      header: "值",
      field: "value",
      label: "配置值",
      renderCell: (s: SettingsDto) => <MyTruncate value={s.value} />,
    },

    {
      accessorKey: "resolved_value",
      header: "已解决",
      field: "resolved_value",
      label: "解析值",
      renderCell: (s: SettingsDto) => <MyTruncate value={s.resolved_value} />,
    },

    {
      accessorKey: "description",
      header: "描述",
      field: "description",
      label: "描述",
      sortBy: true,
    },
    {
      accessorKey: "type",
      header: "类型",
      field: "type",
      label: "类型",
      sortBy: true,
    },
    {
      accessorKey: "updated_at",
      header: "更新时间",
      field: "updated_at",
      sortBy: true,
      renderCell: (row: SettingsDto) => {
        return <span>{DatetimeToShow(row.updated_at)}</span>;
      },
    },
  ];
  const mutationSetting = useReactive<Partial<SettingsDto>>({
    key: "",
    value: "",
    type: SettingValueType.String,
    description: "",
    protected: true,
  });
  const mutationColumns = [
    {
      header: "键",
      field: "key",
      label: "配置键",
      render: (
        <TextInput
          value={mutationSetting.key}
          onChange={(e) => {
            mutationSetting.key = e.target.value;
          }}
        />
      ),
    },
    {
      header: "value",
      field: "value",
      label: "配置值",
      render: (
        <TextInput
          value={mutationSetting.value}
          onChange={(e) => {
            mutationSetting.value = e.target.value;
          }}
        />
      ),
    },
    {
      header: "描述",
      field: "description",
      label: "描述",
      render: (
        <TextInput
          value={mutationSetting.description}
          onChange={(e) => {
            mutationSetting.description = e.target.value;
          }}
        />
      ),
    },
    {
      header: "type",
      field: "type",
      label: "类型",
      render: (
        <Select
          value={mutationSetting.type}
          onChange={(e) => {
            mutationSetting.type = e.target.value as SettingValueType;
          }}
        >
          {Object.values(SettingValueType).map((type) => (
            <Select.Option key={type} value={type}>
              {type}
            </Select.Option>
          ))}
        </Select>
      ),
    },
    {
      header: "受保护",
      field: "protected",
      label: "受保护",
      render: (
        <Stack direction="horizontal" align="center">
          <ToggleSwitch
            aria-labelledby="default-toggle-label"
            checked={mutationSetting.protected}
            onClick={() => {
              mutationSetting.protected = !mutationSetting.protected;
            }}
          />
        </Stack>
      ),
    },
  ];
  return (
    <>
      {/* 已安装前端选择器（Frontend != Theme）：只选择已安装前端，写入 FRONTEND_ACTIVE。
          安装/升级属于运维 CLI（frontend.sh），浏览器不参与。 */}
      <FrontendSelector />
      <GenericTable
        subject={subject}
        columns={columns}
        mutationColumns={mutationColumns}
        mutationData={mutationSetting}
        queryFn={adminApi.settings.fetch}
        createFn={adminApi.settings.create}
        removeFn={adminApi.settings.remove}
        patchFn={adminApi.settings.patch}
        disablePagination={true}
      />
    </>
  );
}
