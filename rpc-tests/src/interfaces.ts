import { Metadata, ProgramId } from '@gear-js/api';
import { u128, u64 } from '@polkadot/types';
import { H256 } from '@polkadot/types/interfaces';

export interface IPayload {
  kind: string;
  value: any;
}

export interface IExpMessage {
  destination: string | number;
  payload: IPayload;
  init?: boolean;
  gas_limit?: u64;
  value?: u128;
}

export interface IFixtureMessage {
  destination: number;
  payload: IPayload;
  source?: string;
  gas_limit?: number;
  value?: number;
}

export interface IExpected {
  step: number;
  messages: IExpMessage[];
  log: IExpMessage[];
  memory?: any[];
}

export interface IFixtures {
  title: string;
  messages?: IFixtureMessage[];
  expected?: IExpected[];
}

export interface ITestData {
  title: string;
  programs: any;
  fixtures: IFixtures[];
  skipRpcTest?: boolean;
}

export interface ITestPrograms {
  [key: number]: ProgramId;
}

export interface ITestMetadata {
  [key: number]: Metadata;
}
