<?php
declare(strict_types=1);

class SettingRecord
{
    public ?int $count = null;
}

class DetailedSettingRecord extends SettingRecord {}

function useSetting(int $count): int
{
    return $count;
}

function selectSetting(SettingRecord|DetailedSettingRecord $record): ?int
{
    return $record->count === null ? null : useSetting($record->count);
}
