<?php
namespace Aye {
    /** @return void */
    function foo() { }
}
namespace Bee {
    use Aye as A;

    A\foo();
}
