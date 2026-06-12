<?php
class A {
    public int $b = 0;
}
$a = new A();
$b = &$a->b;
$b = ''; // Fatal error
