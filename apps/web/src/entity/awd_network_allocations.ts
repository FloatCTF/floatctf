import type { AwdNetworkAllocationKind } from './sea_orm_active_enums';

export type AwdNetworkAllocations = {
  id: string;
  event_id: string;
  kind: AwdNetworkAllocationKind;
  cidr: string;
  allocated_at: string;
  released_at?: string;
};
