import { ChakraProvider } from '@chakra-ui/react'
import { BrowserRouter, Route, Routes } from 'react-router-dom'
import { QueryParamProvider } from 'use-query-params'
import { ReactRouter6Adapter } from 'use-query-params/adapters/react-router-6'
import { parse, stringify } from 'query-string'
import HomePage from './pages/HomePage'
import SpotifyImplicitGrantLoginPage from './pages/SpotifyImplicitGrantLoginPage'
import SpotifyImplicitGrantCallbackPage from './pages/SpotifyImplicitGrantCallbackPage'

function App() {
  return (
    <BrowserRouter>
      <QueryParamProvider
        adapter={ReactRouter6Adapter}
        options={{
          searchStringToObject: parse,
          objectToSearchString: stringify
        }}
      >
        <ChakraProvider>
          <Routes>
            <Route path="/" element={<HomePage />}></Route>
            <Route
              path="/login"
              element={<SpotifyImplicitGrantLoginPage />}
            ></Route>
            <Route
              path="/login-callback"
              element={<SpotifyImplicitGrantCallbackPage />}
            />
            <Route path="/*" element={<h1>Not found</h1>} />
          </Routes>
        </ChakraProvider>
      </QueryParamProvider>
    </BrowserRouter>
  )
}

export default App
