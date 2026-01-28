<?php
class C {
    /** @var int */
    private $id = 42;
}
$r = array_column([new C], "id");
            
