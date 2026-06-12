<?php
function getOutput(): resource {
    $res = fopen("php://output", "w");

    if ($res === false) {
        throw new \Exception("Cannot write");
    }

    return $res;
}
