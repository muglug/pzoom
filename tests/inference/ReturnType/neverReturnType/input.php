<?php
function exitProgram(bool $die): never
{
    if ($die) {
        die;
    }

    exit;
}

function throwError(): never
{
    throw new Exception();
}
