<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo {
                            const AA = "a";
                        }
                    }
                    namespace B {
                        class Bat {
                            public function batBat() : void {
                                echo \A\Foo::AA;
                            }
                        }
                    }
