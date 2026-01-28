<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo {
                            public function barBar(): void {
                            }
                        }

                        function getFoo(): Foo {
                            return new Foo();
                        }
                    }

                    namespace A\B {
                        class Bat {
                            public function batBat(): void {
                                \A\getFoo()->barBar();
                            }
                        }
                    }
