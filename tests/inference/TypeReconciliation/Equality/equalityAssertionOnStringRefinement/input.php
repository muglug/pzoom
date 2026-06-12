<?php
/** @param non-empty-lowercase-string $id */
function f(string $id): void {
    if (strpos($id, '::__construct')) {
        echo "yes";
    }
}
