<?php
/** @return false|null */
function bar() {
    return rand(0, 5) ? null : false;
}

if (empty(bar())) {
    echo "abc";
}
