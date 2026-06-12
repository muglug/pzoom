<?php
enum Code: int
{
    case Ok = 0;
    case Fatal = 1;
}

function foo(): int|Code|null
{
    return null;
}

$code = foo();
if(!isset($code)){
    $code = Code::Ok;
}
