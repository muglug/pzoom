<?php
function a(array $arr) : void {
    /** @psalm-suppress MixedArgument */
    echo isset($arr["a"]["b"]) ? $arr["a"]["b"] : 0;
}

function b(array $arr) : void {
    /** @psalm-suppress MixedArgument */
    echo $arr["a"]["b"] ?? 0;
}