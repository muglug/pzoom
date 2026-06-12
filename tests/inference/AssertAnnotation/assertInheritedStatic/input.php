<?php
    class A {
        /** @var null|int */
        protected static $q = null;
    }

    class B extends A {
        /** @psalm-assert int self::$q */
        protected static function prefillQ(): void {
            self::$q = 123;
        }
        public static function getQ(): int {
            self::prefillQ();
            return self::$q;
        }
    }
?>
