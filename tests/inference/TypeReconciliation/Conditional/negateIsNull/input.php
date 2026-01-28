<?php
function scope(?string $str): string{
    if (is_null($str) === false){
        return $str;
    }

    return "";
}