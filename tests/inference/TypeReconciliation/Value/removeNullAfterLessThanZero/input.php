<?php
function fcn(?int $val): int {
    if ($val < 0) {
        return $val;
    }

    return 5;
}