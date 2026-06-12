<?php
function takesBool(bool $_b): void {}

/** @param array<string, mixed> $options */
function f(array $options): void {
    if (isset($options['x'])) {
        $b = filter_var(
            $options['x'],
            FILTER_VALIDATE_BOOLEAN,
            ['flags' => FILTER_NULL_ON_FAILURE],
        );
        if ($b === null) {
            return;
        }
        takesBool($b);
    }
}
