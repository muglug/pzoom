<?php
function test(string|int|float|bool $value): bool {
    if (is_numeric($value) || $value === true) {
        if ($value === true || (int) $value !== 0) {
            return true;
        }
    }
    return false;
}
