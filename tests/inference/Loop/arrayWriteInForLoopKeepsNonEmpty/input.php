<?php
final class U5 {}
/** @param non-empty-array<int|string, U5> $props */
function takesProps(array $props): void { echo count($props); }

/**
 * @param non-empty-array<int|string, U5> $properties
 * @param int<0, max> $min
 * @param int<0, max> $count
 */
function f(array $properties, int $min, int $count): void {
    for ($i = $min; $i < $count; $i++) {
        $properties[$i] = new U5();
    }
    takesProps($properties);
}
