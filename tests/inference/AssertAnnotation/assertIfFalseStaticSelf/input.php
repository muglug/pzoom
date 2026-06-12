<?php
    final class C {
        /** @var null|int */
        private static $q = null;

        /** @psalm-assert-if-false int self::$q */
        private static function prefillQ(): bool {
            if (rand(0,1)) {
                self::$q = 123;
                return false;
            }
            return true;
        }

        public static function getQ(): int {
            if (self::prefillQ()) {
                return -1;
            }
            return self::$q;
        }
    }
?>
