<?php
function takesAString(string $name): void {}

function randomReturn(): ?string {
    return rand(1,2) === 1 ? "foo" : null;
}

$name = randomReturn();

if ($foo = ($name !== null)) {
    takesAString($name);
}