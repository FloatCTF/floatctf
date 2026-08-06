import type { UniResponse } from "@/api/axios";
import { XIcon } from "@primer/octicons-react";
import { Autocomplete, Button, FormControl, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
export type add_params = {
  event_id: string;
  ids?: string[];
  id?: string;
};
interface ActionSelectProps<T> {
  event_id: string;
  label: string; // 作为 queryKey 前缀
  maxHeight?: number;
  buttonText: string; // 按钮文字 ("Add" / "Open")
  queryKey?: string; // 可选的 queryKey

  // biome-ignore lint/suspicious/noExplicitAny: <explanation>
  mutationFn: (params: add_params) => Promise<UniResponse<any>>;
  fetchFn: () => Promise<UniResponse<T[]>>; // 通用获取数据函数
  itemText: (item: T) => string; // 渲染 item 显示的文本
  getId: (item: T) => string; // 提取 id
  onChange?: (selected: T[]) => void;
  enableImportJson?: boolean; // 是否支持 JSON 导入
}

export const ActionSelect = <T,>({
  event_id,
  label,
  maxHeight = 240,
  buttonText,
  queryKey,
  mutationFn,
  fetchFn,
  itemText,
  getId,
  onChange,
  enableImportJson = false,
}: ActionSelectProps<T>) => {
  const queryClient = useQueryClient();

  // 获取数据
  const { data } = useQuery({
    queryKey: [label, event_id], // ✅ label+id 作为 queryKey
    queryFn: fetchFn,
  });
  const itemsData = data?.data ?? [];

  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [json, setJson] = useState("");

  const items = itemsData.map((i) => ({
    id: getId(i),
    text: itemText(i),
  }));

  const selectedItems = itemsData.filter((i) => selectedIds.includes(getId(i)));

  const mutation = useMutation({
    mutationFn,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: [label, event_id] });
      if (queryKey) {
        queryClient.invalidateQueries({ queryKey: [queryKey] });
      }
      setSelectedIds([]);
    },
  });

  return (
    <FormControl>
      <FormControl.Label id={`${label}-list`}>{label}</FormControl.Label>
      <Autocomplete>
        <Autocomplete.Input
          aria-labelledby={`${label}-list`}
          placeholder={`Chosen ${selectedIds.length} items`}
        />
        {enableImportJson && selectedItems.length === 0 && (
          <div className="flex flex-col gap-2">
            <TextInput
              value={json}
              onChange={(e) => setJson(e.target.value)}
              placeholder={`${label}.json`}
            />
            <Button
              variant="primary"
              onClick={() => {
                const parsed = JSON.parse(json);
                mutation.mutate({
                  event_id,
                  ids: parsed.map((c: T) => getId(c)),
                });
              }}
            >
              Import
            </Button>
          </div>
        )}
        {selectedItems.length > 0 && (
          <Button
            variant="primary"
            onClick={() => {
              mutation.mutate({
                event_id,
                ids: selectedIds,
              });
            }}
          >
            {buttonText}
          </Button>
        )}
        <Autocomplete.Overlay>
          <Autocomplete.Menu
            selectionVariant="multiple"
            items={items}
            selectedItemIds={selectedIds}
            onSelectedChange={(selected) => {
              if (!Array.isArray(selected)) return;
              const ids = selected.map((s) => s.id!);
              setSelectedIds(ids);
              onChange?.(itemsData.filter((i) => ids.includes(getId(i))));
            }}
            aria-labelledby={`${label}-list`}
            // @ts-ignore
            style={{ maxHeight, overflowY: "auto" }}
          />
        </Autocomplete.Overlay>
      </Autocomplete>

      {/* 展示已选中的项 */}
      {selectedItems.length > 0 && (
        <ul style={{ marginTop: 8 }}>
          {selectedItems.map((i) => (
            <li key={getId(i)}>
              {itemText(i)}{" "}
              <button
                type="button"
                onClick={() => {
                  const ids = selectedIds.filter((id) => id !== getId(i));
                  setSelectedIds(ids);
                  onChange?.(itemsData.filter((j) => ids.includes(getId(j))));
                }}
              >
                <XIcon />
              </button>
            </li>
          ))}
        </ul>
      )}
    </FormControl>
  );
};
