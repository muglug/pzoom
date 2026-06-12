<?php
/** @return never */
function neverReturns() {
    die();
}

function f(): void {
    neverReturns();
    echo "hello";
}

