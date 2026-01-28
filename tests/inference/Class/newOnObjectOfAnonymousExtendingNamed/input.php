<?php
function f(): Exception {
    $o = new class extends Exception {};
    return new $o;
}
