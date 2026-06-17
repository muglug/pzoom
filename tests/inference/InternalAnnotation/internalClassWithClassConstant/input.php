<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo {
                        }
                    }
                    namespace B {
                        class Bat {
                            public function batBat() : string {
                                return \A\Foo::class;
                            }
                        }
                    }
