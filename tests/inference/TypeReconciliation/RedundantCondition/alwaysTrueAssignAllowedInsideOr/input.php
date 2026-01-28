<?php
class A{
    public function get(): ?stdClass{ return new stdClass;}
}
$a = new A();

if ($a->get() || ($c = rand(0,1))){


}