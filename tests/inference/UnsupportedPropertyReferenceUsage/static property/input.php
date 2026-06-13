<?php
class A {
    public static int $b = 0;
}
$b = &A::$b;
$b = ''; // Fatal error
