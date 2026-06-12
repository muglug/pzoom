<?php
final class ArgsHelper {
    public const FILTERLIKE = ['array_filter', 'array_find'];
}
final class ArgChecker {
    private const NON_PUBLIC_CB = [
        ...ArgsHelper::FILTERLIKE,
        'array_map',
        'usort',
    ];
    public static function check(string $name): bool {
        return in_array($name, self::NON_PUBLIC_CB, true);
    }
}
