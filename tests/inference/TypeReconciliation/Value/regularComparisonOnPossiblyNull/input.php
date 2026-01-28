<?php
/** @psalm-ignore-nullable-return */
function generate() : ?string {
    return rand(0, 1000) ? "hello" : null;
}

function foo() : string {
    $str = generate();

    if ($str[0] === "h") {
        return $str;
    }

    return "hello";
}