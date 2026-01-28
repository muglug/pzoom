<?php
/** @return void */
function foo(Exception $e) {
    switch (get_class($e)) {
        case "InvalidArgumentException":
            break;
    }
}
