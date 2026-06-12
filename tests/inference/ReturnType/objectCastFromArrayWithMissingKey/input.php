<?php
/** @return object{status: string} */
function foo(): object {
    return (object) [
        "notstatus" => "failed",
    ];
}
