<?php
function foo(DateTimeImmutable $fooDate): string
{
    return $fooDate->format("Y");
}

foo(max(new DateTimeImmutable(), new DateTimeImmutable()));
