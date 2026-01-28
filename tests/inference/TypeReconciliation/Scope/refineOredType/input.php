<?php
class A {
    public function doThing(): void
    {
        if ($this instanceof B || $this instanceof C) {
            if ($this instanceof B) {

            }
        }
    }
}
class B extends A {}
class C extends A {}