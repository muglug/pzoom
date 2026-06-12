<?php
/** @return resource */
function getOutput() {
    $res = fopen("php://output", "w") or die();

    return $res;
}
