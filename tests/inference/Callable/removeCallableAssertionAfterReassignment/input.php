<?php
function foo(string $key) : void {
    $setter = "a" . $key;
    if (is_callable($setter)) {
        return;
    }
    $setter = "b" . $key;
    if (is_callable($setter)) {}
}
