<?php
/**
 * @return array<string, null>
 */
function foo(): array {
    $array = [];
    /** @psalm-suppress NullArrayOffset */
    $array[null] = null;
    return $array;
}
