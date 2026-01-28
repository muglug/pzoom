<?php
class G
{
    /**
     * @psalm-if-this-is G
     * @return void
     */
    public function test() {}
}

class F extends G
{
}

$f = new F();
$f->test();
