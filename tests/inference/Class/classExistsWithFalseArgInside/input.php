<?php
function foo(string $s) : void {
    if (class_exists($s, false)) {
        /** @psalm-suppress MixedMethodCall */
        new $s();
    }
}
