<?php
namespace Foo {
    /**
     * @method int randomInt()
     * @method void takesFloat($a = 0.1)
     */
    class G
    {
        /**
         * @param string $method
         * @param array $attributes
         *
         * @return mixed
         */
        public function __call($method, $attributes)
        {
            return null;
        }
    }
}

namespace Bar {
    (new \Foo\G)->randomInt();
}
