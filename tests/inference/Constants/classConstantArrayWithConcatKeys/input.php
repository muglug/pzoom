<?php
namespace Foo;

final class CodeLocation
{
    private const PROPERTY_KEYS = [
        'file_path' => 'file_path',
        "\0" . self::class . "\0" . 'end_line' => 'end_line',
        "\0*\0" . 'file_start' => 'file_start',
    ];

    /** @param array<string, mixed> $properties */
    public function load(array $properties): void
    {
        foreach (self::PROPERTY_KEYS as $key => $property_name) {
            echo $key;
            echo $property_name;
        }
    }
}
