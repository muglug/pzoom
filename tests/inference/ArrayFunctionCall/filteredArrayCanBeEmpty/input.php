<?php
/**
  * @return string|null
  */
function thing() {
    if(rand(0,1) === 1) {
        return "data";
    } else {
        return null;
    }
}
$list = [thing(),thing(),thing()];
$list = array_filter($list);
if (!empty($list)) {}
