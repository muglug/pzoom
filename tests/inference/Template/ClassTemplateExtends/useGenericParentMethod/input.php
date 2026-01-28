<?php
/**
 * @template-extends ArrayObject<string, string>
 */
class Foo extends ArrayObject
{
    public function bar() : void {
        $c = $this->getArrayCopy();
        foreach ($c as $d) {
            echo $d;
        }
    }
}