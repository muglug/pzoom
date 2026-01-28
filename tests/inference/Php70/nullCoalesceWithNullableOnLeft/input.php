<?php
/** @return ?string */
function foo() {
    return rand(0, 10) > 5 ? "hello" : null;
}
$a = foo() ?? "goodbye";
