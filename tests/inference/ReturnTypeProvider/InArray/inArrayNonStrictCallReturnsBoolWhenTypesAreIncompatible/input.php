<?php
/**
 * @return string[]
 */
function f(): array {
    return ["1"];
}
$ret = in_array(1, f());
            
