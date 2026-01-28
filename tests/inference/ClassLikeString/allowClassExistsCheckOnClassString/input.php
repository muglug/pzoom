<?php
class C
{
    public function __construct() {
        if (class_exists(\Doesnt\Really::class)) {
            \Doesnt\Really::something();
        }
    }
}
