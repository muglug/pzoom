<?php
/** @return list<int>|false */
function getRows() {
    return rand(0, 1) ? [1, 2, 3] : false;
}

foreach (getRows() as $row) {
    echo $row;
}
