<?php
/** @return list<int>|null */
function getRows() {
    return rand(0, 1) ? [1, 2, 3] : null;
}

foreach (getRows() as $row) {
    echo $row;
}
