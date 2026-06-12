<?php
function foo(mixed $var) : void {
    switch (get_debug_type($var)) {
        case "string":
            echo "a string";
            break;

        case Exception::class;
            echo "an Exception with message " . $var->getMessage();
            break;
    }
}
