<?php
function foo(Exception $e) : void {
    if (get_class($e) == "InvalidArgumentException") {}
}
