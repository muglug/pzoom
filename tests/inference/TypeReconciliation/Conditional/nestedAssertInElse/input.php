<?php
function foo(string $type, bool $and) : void {
    if ($type === "a") {
    } elseif ($type === "b" && $and) {
    } else {
        if ($type === "c" && $and) {}
    }
}