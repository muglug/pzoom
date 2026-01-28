<?php
if (class_exists(My_Parent::class) && !class_exists(My_Extend::class)) {
    /**
     * Extended class
     */
    class My_Extend extends My_Parent {
        /**
         * Construct
         *
         * @return void
         */
        public function __construct() {
            echo "foo";
        }
    }
}
