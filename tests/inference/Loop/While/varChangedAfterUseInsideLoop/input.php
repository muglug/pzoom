<?php
function takesString(string $s) : void {}

/**
 * @param array<string> $fields
 */
function changeVarAfterUse(array $values, array $fields): void {
    foreach ($fields as $field) {
        if (!isset($values[$field])) {
            continue;
        }

        /** @psalm-suppress MixedAssignment */
        $value = $values[$field];

        /** @psalm-suppress MixedArgument */
        takesString($value);

        $values[$field] = null;
    }
}
