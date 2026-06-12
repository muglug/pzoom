<?php
function f(string $v): void {
    if (($string_to_float = filter_var($v, FILTER_VALIDATE_FLOAT)) === false) {
        echo 'bad';
        return;
    }
    echo $string_to_float * 2.0;
}
