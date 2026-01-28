<?php
function x($foo, string $bar): void {
    if (!in_array($foo, [$bar], true)) {
        throw new Exception();
    }

    if (is_string($foo)) {}
}
