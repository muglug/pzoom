<?php
function foo(callable $c): void {
    echo (string)$c();
}
