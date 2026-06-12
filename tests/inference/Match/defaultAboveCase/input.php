<?php
function foo(string $a) : string {
    return match ($a) {
        "a" => "hello",
        default => "yellow",
        "b" => "goodbye",
    };
}
