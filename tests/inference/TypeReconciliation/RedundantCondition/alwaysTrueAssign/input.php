<?php
class A{
    public function get(): stdClass{ return new stdClass;}
}
$a = new A();

if ($c = $a->get()){


}
                    
