<?php
function f(): object {
    $o = new class {};
    return new $o;
}
