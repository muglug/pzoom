<?php
function call(object $obj): void {
    if (!method_exists($obj, "__invoke")) {
        return;
    }
    $obj();
}
