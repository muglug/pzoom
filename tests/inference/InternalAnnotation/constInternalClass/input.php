<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo {
                            const AA = "a";
                        }

                        class Bat {
                            public function batBat() : void {
                                echo \A\Foo::AA;
                            }
                        }
                    }
