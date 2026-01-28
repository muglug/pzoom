<?php
function maybeString(): ?string {
    return rand(0, 10) > 4 ? "test" : null;
}

function test(): string {
    $foo = maybeString();
    ($foo === null) && ($foo = "");

    return $foo;
}