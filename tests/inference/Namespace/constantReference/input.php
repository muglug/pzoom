<?php
namespace Aye\Bee {
    const HELLO = "hello";
}
namespace Aye\Bee {
    /** @return void */
    function foo() {
        echo \Aye\Bee\HELLO;
    }

    class Bar {
        /** @return void */
        public function foo() {
            echo \Aye\Bee\HELLO;
        }
    }
}