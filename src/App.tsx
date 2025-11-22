import "./App.css";
import "react";
import { BrowserRouter, useRoutes } from "react-router-dom";

import routes from "~react-pages";

function Routes() {
  return useRoutes(routes);
}

function App() {
  console.log(routes);

  return (
    <BrowserRouter>
      <Routes />
    </BrowserRouter>
  );
}

export default App;
