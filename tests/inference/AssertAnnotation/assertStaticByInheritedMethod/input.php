<?php
    class A {
        /** @var null|int */
        protected static $q = null;

        /** @psalm-assert int self::$q */
        protected static function prefillQ(): void {
            self::$q = 123;
        }
    }

    class B extends A {
        public static function getQ(): int {
            self::prefillQ();
            return self::$q;
        }
    }
?>
