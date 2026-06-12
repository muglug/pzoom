<?php
function scope(array $a): int|float {
    $offset = array_search("foo", $a);
    if(is_numeric($offset)){
        return $offset++;
    }
    else{
        return 0;
    }
}
