<?php
/** @param array<string, int|string> $map */
function foo(array $map, string $o) : void {
    if ($mapped_type = $map[$o] ?? null) {
        if (is_int($mapped_type)) {
            return;
        }
    }

    if (($mapped_type = $map[""] ?? null) && is_string($mapped_type)) {

    }
}