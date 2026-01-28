<?php
namespace Foo;

/**
 * @return (func_num_args() is 0 ? false : string)
 */
function zeroArgsFalseOneArgString(string $s = "") {
    if (func_num_args() === 0) {
        return false;
    }

    return $s;
}

$a = zeroArgsFalseOneArgString("hello");