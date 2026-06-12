<?php
namespace Psalm\Internal;

final class VersionUtils
{
    public static function getPsalmVersion(): string
    {
        return '1.0';
    }
}

final class CliUtils
{
    public static function initialize(): void
    {
        define('PSALM_VERSION', VersionUtils::getPsalmVersion());
    }
}

function takesString(string $s): void
{
    echo $s;
}

function usesConstant(): void
{
    takesString(PSALM_VERSION);
}
