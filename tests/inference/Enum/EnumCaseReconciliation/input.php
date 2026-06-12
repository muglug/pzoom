<?php
enum Code: int
{
    case Ok = 0;
    case Fatal = 1;
}

function foo(): Code|null
{
    return null;
}

$code = foo();
$code1 = null;
$code2 = null;
if($code instanceof Code){
    $code1 = $code;
}
if(!$code instanceof Code){
    $code2 = $code;
}
