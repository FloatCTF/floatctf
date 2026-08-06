import { Select, Stack, TextInput, ToggleSwitch } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import { MyTruncate } from "@/components/Truncate";
import { SettingValueType, type Settings } from "@/entity";
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
      header: "Key",
      field: "key",
      sortBy: true,
    },
    {
      accessorKey: "value",
      header: "Value",
      field: "value",
      renderCell: (s: Settings) => <MyTruncate value={s.value} />,
    },

    {
      accessorKey: "description",
      header: "Description",
      field: "description",
      sortBy: true,
    },
    {
      accessorKey: "type",
      header: "Type",
      field: "type",
      sortBy: true,
    },
    {
      accessorKey: "updated_at",
      header: "Updated At",
      field: "updated_at",
      sortBy: true,
      renderCell: (row: Settings) => {
        return <span>{DatetimeToShow(row.updated_at)}</span>;
      },
    },
  ];
  const mutationSetting = useReactive<Partial<Settings>>({
    key: "",
    value: "",
    type: SettingValueType.String,
    description: "",
    protected: true,
  });
  const mutationColumns = [
    {
      header: "key",
      field: "key",
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
      header: "description",
      field: "description",
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
      header: "protected",
      field: "protected",
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
  );
}
