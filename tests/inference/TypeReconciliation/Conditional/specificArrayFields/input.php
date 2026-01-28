<?php
/**
 * @param array{field:string, ...} $array
 */
function print_field($array) : void {
    echo $array["field"];
}

/**
 * @param array{field:string,otherField:string} $array
 */
function has_mix_of_fields($array) : void {
    print_field($array);
}
