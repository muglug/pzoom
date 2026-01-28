<?php
/**
 * @param array{field:string, otherField:string} $array
 */
function print_field($array) : void {
    echo $array["field"] . " " . $array["otherField"];
}

print_field(["field" => "name"]);
